using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using FixMath;

public class DoSkill : MonoBehaviour
{
    public int singing;
    public delegate void PointSkill(Fix64Vector2 actionplace);
    public PointSkill Fire;
    public delegate void NoPointSkill();
    public NoPointSkill ClearDebuff;
    public NoPointSkill WorkBeforeDestroy;

    // Use this for initialization
    void Start ()
    {
        Fire = null;
        ClearDebuff = null;
    }

    public void justdoit(Fix64Vector2 fv2)
    {
        if (Fire == null)
            return;
        Fire(fv2);
        FireReset();
    }

    public void FireStop()
    {
        gameObject.GetComponent<MoveScript>().stopwalking();
        singing = 0;
        FireReset();
    }

    public void FireReset()
    {
        Fire = null;
    }

    public void DoClearJob()
    {
        if (ClearDebuff == null)
            return;
        ClearDebuff();
        ClearDebuff = null;
    }

    public void DestroyClean()
    {
        if (WorkBeforeDestroy == null)
            return;
        WorkBeforeDestroy();
        WorkBeforeDestroy = null;
    }

    public void BeforeSkill()
    {
        gameObject.GetComponent<MoveScript>().stopwalking(); //停止走动
        //gameObject.GetComponent<StealthScript>().StealthEnd();
        //gameObject.GetComponent<SkillE2b>().lightninghit();
    }
}
