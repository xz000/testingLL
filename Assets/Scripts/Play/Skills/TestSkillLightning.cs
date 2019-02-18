using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using FixMath;

public class TestSkillLightning : MonoBehaviour
{
    public CooldownImage MyImageScript;
    public float maxdistance = 10;
    private float currentcooldown;
    public float cooldowntime = 3;
    public bool skillavaliable;
    public float SelfR = 0.51f;
    Fix64 SRF;
    public LineRenderer line;

    // Use this for initialization
    void Start()
    {
        currentcooldown = cooldowntime;
        SRF = (Fix64)SelfR;
    }

    // Update is called once per frame
    public void GoTestSkillLightning()
    {
        if (skillavaliable && GetComponent<DoSkill>().CanSing)
        {
            GetComponent<DoSkill>().singing = 0;
            gameObject.GetComponent<DoSkill>().Fire = Skill;
        }
    }

    private void FixedUpdate()
    {
        if (skillavaliable)
            return;
        if (currentcooldown >= cooldowntime)
        {
            skillavaliable = true;
        }
        else
        {
            currentcooldown += Time.fixedDeltaTime;
            //MyImageScript.IconFillAmount = currentcooldown / cooldowntime;
        }
    }

    public void Skill(Fix64Vector2 actionplace)
    {
        Fix64Vector2 realplace;
        Vector2 rpv2;
        GetComponent<DoSkill>().BeforeSkill();
        currentcooldown = 0;
        skillavaliable = false;
        //gameObject.GetComponent<DoSkill>().Fire = null;
        Rigidbody2D selfrb2d = gameObject.GetComponent<Rigidbody2D>();
        Fix64Vector2 selfv2f = (Fix64Vector2)selfrb2d.position;
        Fix64Vector2 skilldirection = actionplace - selfv2f;
        RaycastHit2D hit2D = Physics2D.Raycast((selfv2f + skilldirection.normalized() * SRF).ToV2(), (skilldirection - skilldirection.normalized() * SRF).ToV2());
        if (hit2D.collider != null && (Fix64)hit2D.distance <= (Fix64)maxdistance - SRF)
        {
            rpv2 = hit2D.point;
            realplace = (Fix64Vector2)rpv2;
            Drawline(rpv2);
            if (hit2D.collider.GetComponent<DestroyScript>() != null && hit2D.collider.GetComponent<DestroyScript>().breakable == true)
                hit2D.collider.GetComponent<DestroyScript>().Destroyself();
            else if (hit2D.collider.GetComponent<RollScript>() != null)
            {
                //if (!hit2D.collider.GetComponent<PhotonView>().isMine)
                    //hit2D.collider.GetComponent<DestroyScript>().Destroyself();
                return;
            }
            else if (hit2D.collider.GetComponent<RBScript>() != null)
            {
                Fix64Vector2 kickdirection = (Fix64Vector2)hit2D.collider.GetComponent<Rigidbody2D>().position - realplace;
                //hit2D.collider.GetComponent<SkillE2b>().lighthit();
                hit2D.collider.GetComponent<RBScript>().GetPushed(kickdirection * (Fix64)6, 1);
                hit2D.collider.GetComponent<HPScript>().GetHurt(10);
            }
        }
        else
        {
            rpv2 = selfrb2d.position + maxdistance * skilldirection.ToV2().normalized;
            Drawline(rpv2);
        }
    }

    void Drawline(Vector2 destn)
    {
        line.SetPosition(0, GetComponent<Rigidbody2D>().position);
        line.SetPosition(1, destn);
        StartCoroutine(EraseLine());
    }

    IEnumerator EraseLine()
    {
        yield return new WaitForSeconds(0.1f);
        line.SetPosition(0, Vector3.zero);
        line.SetPosition(1, Vector3.zero);
    }

    void TestSkillLightningSetLevel(int i)
    {
        if (i == 0)
            enabled = false;
        else
            enabled = true;
    }

    public float CalcFA()
    {
        return currentcooldown / cooldowntime;
    }
}
