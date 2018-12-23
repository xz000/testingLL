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
    public Toggle RToggle;
    public string netFileName;
    public string nettxtPath;
    public float netFrameLength = 0.1f;
    float netCurrentLength = 0;
    public int PassedFrameNum = -1;
    public int ReceivedFrameNum = 0;
    public int LocalFrameNum = 1;
    public float LocalFrameLength = 0.1f;
    public float LocalCurrentLength = 0;
    public List<ClickData> L2S = new List<ClickData>();
    public static int channelID;
    byte[] buffer2s = new byte[1024];
    public bool isstarted = false;
    //public bool Fstarted = false;
    public byte error;
    uint mn = 1;
    LoopList theLL;

    private void FixedUpdate()
    {
        /*if (!Fstarted)
            return;*/
        netCurrentLength += Time.fixedDeltaTime;
        while (netCurrentLength >= netFrameLength)
        {
            if (theLL.Numready(mn))
            {
                theLL.printhead();
                netCurrentLength -= netFrameLength;
                PassedFrameNum++;
                mn = 0;
            }
            else
            {
                Time.timeScale = 0;
                mn = 1;
                return;
            }
            //Debug.Log("pfn:" + PassedFrameNum);
        }
    }

    private void Update()
    {
        if (!isstarted)
            return;
        if ((LocalFrameNum - PassedFrameNum) < 5)
        {
            if (!RToggle.isOn)
                RToggle.isOn = true;
            LocalCurrentLength += Time.unscaledDeltaTime;
        }
        else
        {
            RToggle.isOn = false;
            LocalCurrentLength += Time.unscaledDeltaTime / 4;
        }
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
            int a = LocalFrameNum - PassedFrameNum - 1;
            theLL.addat(a, Sender.clientNum, L2S);
            if (Time.timeScale == 0 && theLL.Numready(3))
            {
                //isstarted = false;
                Time.timeScale = 1;
            }
            //
            L2S.Clear();
            buffer2s = new byte[1024];
            LocalCurrentLength -= LocalFrameLength;
            LocalFrameNum++;
        }
    }

    private void OnEnable()
    {
        theLL = new LoopList();
        theLL.init(GetComponent<ControllerScript>());
        PassedFrameNum = 0;
        ReceivedFrameNum = 0;
        LocalFrameNum = 1;
        isstarted = true;
        //Fstarted = true;
        Time.timeScale = 0;
    }

    private void OnDisable()
    {
        isstarted = false;
        //Fstarted = false;
        PassedFrameNum = -1;
        ReceivedFrameNum = 0;
        LocalFrameNum = 1;
        //theLL.stop();
    }

    public void Eat(byte[] bRC)
    {
        BinaryFormatter ef = new BinaryFormatter();
        Stream S2E = new MemoryStream(bRC);
        Data2S datarc = (Data2S)ef.Deserialize(S2E);
        ReceivedFrameNum = datarc.frameNum;
        int a = ReceivedFrameNum - PassedFrameNum - 1;
        theLL.addat(a, datarc.clientNum, datarc.clickDatas);
        if (Time.timeScale == 0 && theLL.Numready(3))
        {
            //isstarted = false;
            Time.timeScale = 1;
        }
    }
}
public class LoopList
{
    private int headnum;
    private int fullnum;
    private List<ClickData>[,] CDA2;
    private bool[,] bool3;
    private ControllerScript CTL;

    public void addat(int a, int b, List<ClickData> lcd)
    {
        if (a >= fullnum)
            jiabei();
        a += headnum;
        if (a >= fullnum)
            a -= fullnum;
        foreach (ClickData cd in lcd)
        {
            CDA2[a, b].Add(cd);
        }
        bool3[a, b] = true;
        bool3[a, 2] = bool3[a, 0] && bool3[a, 1];
    }

    public bool headready()
    {
        return bool3[headnum, 2];
    }

    public bool Numready(uint u)
    {
        bool a = true;
        for(uint i= 0; i <= u; i++)
        {
            a = a && bool3[headnum + i, 2];
        }
        return a;
    }

    public void printhead()
    {
        CTL.powerrr(0, CDA2[headnum, 0]);
        CTL.powerrr(1, CDA2[headnum, 1]);
        for (int n = 0; n < 3; n++)
        {
            bool3[headnum, n] = false;
        }
        headnum++;
        if (headnum >= fullnum)
            headnum -= fullnum;
    }

    void jiabei()
    {
        int nextnum = fullnum * 2;
        int endingnum = fullnum + headnum;
        List<ClickData>[,] nextlist = new List<ClickData>[nextnum, 2];
        bool[,] nextb3 = new bool[nextnum, 3];
        for (int i = headnum; i < endingnum; i++)
        {
            for (int n = 0; n < 2; n++)
            {
                if (i >= fullnum)
                {
                    int m = i - fullnum;
                    nextlist[i, n] = CDA2[m, n];
                    nextb3[i, n] = bool3[m, n];
                }
                else
                {
                    nextlist[i, n] = CDA2[i, n];
                    nextb3[i, n] = bool3[i, n];
                }
            }
        }
        CDA2 = nextlist;
        bool3 = nextb3;
        fullnum = nextnum;
    }

    public void init(ControllerScript TCS)
    {
        CTL = TCS;
        CTL.createPCs(2);
        headnum = 0;
        fullnum = 32;
        CDA2 = new List<ClickData>[fullnum, 2];
        bool3 = new bool[fullnum, 3];
        for (int i = 0; i < fullnum; i++)
        {
            for (int n = 0; n < 2; n++)
            {
                CDA2[i, n] = new List<ClickData>();
                bool3[i, n] = false;
            }
            bool3[i, 2] = false;
        }
    }
}