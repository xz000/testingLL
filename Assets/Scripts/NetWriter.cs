using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;
using UnityEngine.Networking;
using System.IO;
using System.Text;
using System.Runtime.Serialization.Formatters.Binary;

public class NetWriter : MonoBehaviour
{
    public string netFileName;
    public string nettxtPath;
    FileStream netfs;
    StreamWriter netSWriter;
    public float netFrameLength = 1f;
    float netCurrentLength = 0;
    public int PassedFrameNum = 0;
    public static int ReceivedFrameNum = 0;
    public int LocalFrameNum = 1;
    public float LocalFrameLength = 1f;
    float LocalCurrentLength = 0;
    public List<ClickData> L2S = new List<ClickData>();
    public byte[] bRC = new byte[1024];
    public List<ClickData> L2R = new List<ClickData>();
    public static int channelID;
    byte[] buffer2s = new byte[1024];
    public bool isstarted = false;
    public byte error;
    public static List<bool> bp = new List<bool>();
    public static List<List<ClickData>> LLCD = new List<List<ClickData>>();

    private void FixedUpdate()
    {
        if (!isstarted)
            return;
        netCurrentLength += Time.fixedDeltaTime;
        while (netCurrentLength >= netFrameLength && bp[0])
        {
            ta();
            netCurrentLength -= netFrameLength;
            PassedFrameNum++;
        }
    }

    private void Update()
    {
        if (!isstarted)
            return;
        LocalCurrentLength += Time.deltaTime;
        while (LocalCurrentLength >= LocalFrameLength)
        {
            Data2S Fd2s = new Data2S();
            Fd2s.frameNum = LocalFrameNum;
            Fd2s.clientNum = Sender.clientNum;
            Fd2s.clickDatas = L2S;
            BinaryFormatter bf = new BinaryFormatter();
            MemoryStream ms = new MemoryStream();
            bf.Serialize(ms, Fd2s);
            buffer2s = ms.GetBuffer();
            NetworkTransport.Send(Sender.HSID, Sender.CNID, channelID, buffer2s, buffer2s.Length, out error);
            //
            int a = (LocalFrameNum - PassedFrameNum - 1) * 2 + Sender.clientNum;
            LLCD[a] = new List<ClickData>(L2S);
            int b = (LocalFrameNum - PassedFrameNum - 1) * 3;
            bp[b + 1 + Sender.clientNum] = true;
            bp[b] = bp[b + 1] && bp[b + 2];
            //
            L2S.Clear();
            buffer2s = new byte[1024];
            LocalCurrentLength -= LocalFrameLength;
            LocalFrameNum++;
        }
    }

    private void OnEnable()
    {
        netFileName = "/" + "net" + string.Format("{0:D2}{1:D2}{2:D2}{3:D2}{4:D2}{5:D2}", System.DateTime.Now.Year, System.DateTime.Now.Month, System.DateTime.Now.Day, System.DateTime.Now.Hour, System.DateTime.Now.Minute, System.DateTime.Now.Second) + ".txt";
        nettxtPath = Application.dataPath + netFileName;
        netfs = new FileStream(nettxtPath, FileMode.OpenOrCreate, FileAccess.ReadWrite);
        netSWriter = new StreamWriter(netfs);
        netSWriter.WriteLine("Started");
        isstarted = true;
    }

    private void OnDisable()
    {
        isstarted = false;
        netSWriter.WriteLine("Stoped");
        netSWriter.Close();
    }

    void ta()
    {
        PrintList(LLCD[0]);
        LLCD.RemoveAt(0);
        PrintList(LLCD[0]);
        LLCD.RemoveAt(0);
        bp.RemoveRange(0, 3);
    }

    void PrintList(List<ClickData> theLS)
    {
        foreach (ClickData cd in theLS)
        {
            netSWriter.WriteLine(cd.ToP());
        }
    }

    public void Eat()
    {
        BinaryFormatter ef = new BinaryFormatter();
        Stream S2E = new MemoryStream(bRC);
        bRC = new byte[1024];
        Data2S datarc = (Data2S)ef.Deserialize(S2E);
        //Debug.Log(datarc.frameNum);
        ReceivedFrameNum = datarc.frameNum;
        //L2R = datarc.clickDatas;
        //
        int a = (ReceivedFrameNum - PassedFrameNum - 1) * 2 + datarc.clientNum;
        LLCD[a] = datarc.clickDatas;
        int b = (ReceivedFrameNum - PassedFrameNum - 1) * 3;
        bp[b + 1 + datarc.clientNum] = true;
        bp[b] = bp[b + 1] && bp[b + 2];
    }
}