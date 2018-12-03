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
    public uint netFrameNum = 0;
    public uint LocalFrameNum = 0;
    public float LocalFrameLength = 1f;
    float LocalCurrentLength = 0;
    public static List<ClickData> L2S;
    public static byte[] bRC = new byte[1024];
    public static List<ClickData> L2R;
    public static int channelID;
    byte[] buffer2s = new byte[1024];
    bool isstarted = false;
    public byte error;

    private void FixedUpdate()
    {
        if (!isstarted)
            return;
        netCurrentLength += Time.fixedDeltaTime;
        while (netCurrentLength >= netFrameLength)
        {
            PrintList(ref ClickCatcher.LS);
            netCurrentLength -= netFrameLength;
            netFrameNum++;
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
            Fd2s.clickDatas = L2S;
            L2S.Clear();
            BinaryFormatter bf = new BinaryFormatter();
            MemoryStream ms = new MemoryStream();
            bf.Serialize(ms, Fd2s);
            buffer2s = ms.GetBuffer();
            //序列化数据
            NetworkTransport.Send(Sender.HSID, Sender.CNID, channelID, buffer2s, buffer2s.Length, out error);
            //发送数据
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
    }

    private void OnDisable()
    {
        netSWriter.WriteLine("Stoped");
        netSWriter.Close();
    }

    void PrintList(ref List<string> theLS)
    {
        while (theLS.Count != 0)
        {
            netSWriter.WriteLine(theLS[0]);
            //netSWriter.WriteLine(netFrameNum + theLS[0]);
            theLS.RemoveAt(0);
        }
    }

    public static void Eat()
    {
        BinaryFormatter ef = new BinaryFormatter();
        Stream S2E = new MemoryStream(bRC);
        bRC = new byte[1024];
        Data2S datarc = (Data2S)ef.Deserialize(S2E);
        L2R = datarc.clickDatas;
    }
}
