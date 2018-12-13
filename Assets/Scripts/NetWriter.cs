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
    public uint PassedFrameNum = 0;
    public static uint ReceivedFrameNum = 0;
    public uint LocalFrameNum = 1;
    public float LocalFrameLength = 1f;
    float LocalCurrentLength = 0;
    public List<ClickData> L2S = new List<ClickData>();
    public byte[] bRC = new byte[1024];
    public List<ClickData> L2R = new List<ClickData>();
    public static int channelID;
    byte[] buffer2s = new byte[1024];
    public bool isstarted = false;
    public byte error;
    public delegate void TakeAction();
    public TakeAction ta;

    public List<ClickData>[] CDarray = new List<ClickData>[32];

    private void FixedUpdate()
    {
        if (!isstarted)
            return;
        netCurrentLength += Time.fixedDeltaTime;
        while (netCurrentLength >= netFrameLength && ReceivedFrameNum != 0)
        {
            ReceivedFrameNum = 0;//temp
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
            Fd2s.clickDatas = L2S;
            BinaryFormatter bf = new BinaryFormatter();
            MemoryStream ms = new MemoryStream();
            bf.Serialize(ms, Fd2s);
            buffer2s = ms.GetBuffer();
            NetworkTransport.Send(Sender.HSID, Sender.CNID, channelID, buffer2s, buffer2s.Length, out error);
            ClickCatcher.LS = new List<ClickData>(L2S);
            //netFrameNum = LocalFrameNum;
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
        if (Sender.isServer)
        {
            ta = null;
            ta += LSAction;
            ta += NWAction;
        }
        else
        {
            ta = null;
            ta += NWAction;
            ta += LSAction;
        }
        netSWriter.WriteLine("Started");
        isstarted = true;
    }

    private void OnDisable()
    {
        isstarted = false;
        netSWriter.WriteLine("Stoped");
        netSWriter.Close();
    }

    public void NWAction()
    {
        PrintList(ref L2R);
    }

    void LSAction()
    {
        PrintList(ref ClickCatcher.LS);
    }

    void PrintList(ref List<ClickData> theLS)
    {
        while (theLS.Count != 0)
        {
            netSWriter.WriteLine(theLS[0].ToP());
            theLS.RemoveAt(0);
        }
    }

    public void Eat()
    {
        BinaryFormatter ef = new BinaryFormatter();
        Stream S2E = new MemoryStream(bRC);
        bRC = new byte[1024];
        Data2S datarc = (Data2S)ef.Deserialize(S2E);
        Debug.Log(datarc.frameNum);
        ReceivedFrameNum = datarc.frameNum;
        L2R = datarc.clickDatas;
    }
}
